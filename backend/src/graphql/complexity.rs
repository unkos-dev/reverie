use std::collections::HashMap;

use juniper::{
    DefaultScalarValue, Definition, InputValue, Operation, OperationType, ScalarValue, SchemaType,
    Selection, Variables,
};

use crate::{error::AppError, services::reader};

pub(super) const LIMIT: usize = 200_000;
pub(super) const DEPTH: usize = 12;
const OPERATION_LIST: usize = 144;
const TOO_COMPLEX: &str = "Query is too complex.";
const TOO_DEEP: &str = "Query is nested too deep.";

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub(super) cost: usize,
    pub(super) depth: usize,
}

pub(super) fn effective_page(first: i32, ceiling: u32) -> Result<i32, AppError> {
    if !(0..=i64::from(ceiling)).contains(&i64::from(first)) {
        return Err(AppError::Validation("Invalid page size.".into()));
    }
    Ok(first)
}

pub(super) fn nested_size(kind: reader::RelationKind, first: i32) -> Result<i32, AppError> {
    effective_page(first, kind.max_first())
}

pub(super) fn pending_size(first: i32) -> Result<i32, AppError> {
    effective_page(first, reader::PENDING_MAX)
}

#[derive(Clone, Copy)]
enum Bound {
    Page(u32),
    Search,
    Labels,
    Suggestions,
    Operations,
}

fn bound(parent: &str, field: &str) -> Option<Bound> {
    use reader::RelationKind as Kind;
    let nested = |kind: Kind| Some(Bound::Page(kind.max_first()));
    match (parent, field) {
        ("Query", "books" | "shelves") => Some(Bound::Page(reader::PAGE_MAX)),
        ("Query", "search") => Some(Bound::Search),
        ("Query", "authorLabels" | "seriesLabels") => Some(Bound::Labels),
        (
            "Query",
            "authorSuggestions"
            | "seriesSuggestions"
            | "genreSuggestions"
            | "moodSuggestions"
            | "tagSuggestions"
            | "publisherSuggestions",
        ) => Some(Bound::Suggestions),
        ("Work", "identifiers") | ("MetadataSnapshot", "workIdentifiers") => {
            nested(Kind::WorkIdentifiers)
        }
        ("Work", "manifestations") => nested(Kind::Manifestations),
        ("Work" | "MetadataSnapshot", "authors") => nested(Kind::Authors),
        ("Work" | "MetadataSnapshot", "contributors") => nested(Kind::Contributors),
        ("Manifestation" | "MetadataSnapshot", "genres") => nested(Kind::Genres),
        ("Manifestation" | "MetadataSnapshot", "moods") => nested(Kind::Moods),
        ("Manifestation" | "MetadataSnapshot", "tags") => nested(Kind::Tags),
        ("Manifestation", "identifiers") | ("MetadataSnapshot", "manifestationIdentifiers") => {
            nested(Kind::Identifiers)
        }
        ("Manifestation", "ratings") => nested(Kind::Ratings),
        ("Shelf", "items") => nested(Kind::ShelfItems),
        ("Series", "works") => nested(Kind::SeriesWorks),
        ("MetadataVersions", "pending") => Some(Bound::Page(reader::PENDING_MAX)),
        ("MetadataSave" | "MetadataOperation", "operations") => Some(Bound::Operations),
        _ => None,
    }
}

type Arguments<'a> = Vec<(&'a str, &'a InputValue<DefaultScalarValue>)>;

struct Analysis<'a> {
    schema: &'a SchemaType<DefaultScalarValue>,
    fragments: HashMap<&'a str, (&'a str, &'a [Selection<'a, DefaultScalarValue>])>,
    variables: &'a Variables<DefaultScalarValue>,
    defaults: HashMap<&'a str, &'a InputValue<DefaultScalarValue>>,
    limits: Limits,
    depth: usize,
    expanding: Vec<&'a str>,
    errors: Vec<String>,
}

impl<'a> Analysis<'a> {
    fn selections(
        &mut self,
        parent: Option<&'a str>,
        set: &'a [Selection<'a, DefaultScalarValue>],
        depth: usize,
    ) -> Option<usize> {
        let mut total = 0_usize;
        for selection in set {
            let cost = match selection {
                Selection::Field(field) => {
                    let field = &field.item;
                    let arguments: Arguments<'a> = field
                        .arguments
                        .iter()
                        .flat_map(|arguments| arguments.item.items.iter())
                        .map(|(name, value)| (name.item, &value.item))
                        .collect();
                    self.field(
                        parent,
                        field.name.item,
                        &arguments,
                        field.selection_set.as_deref().unwrap_or_default(),
                        depth,
                    )?
                }
                Selection::FragmentSpread(spread) => {
                    let name = spread.item.name.item;
                    let Some(&(condition, set)) = self.fragments.get(name) else {
                        continue;
                    };
                    if self.expanding.contains(&name) {
                        continue;
                    }
                    self.expanding.push(name);
                    let cost = self.selections(Some(condition), set, depth);
                    self.expanding.pop();
                    cost?
                }
                Selection::InlineFragment(inline) => {
                    let condition = inline
                        .item
                        .type_condition
                        .as_ref()
                        .map(|condition| condition.item)
                        .or(parent);
                    self.selections(condition, &inline.item.selection_set, depth)?
                }
            };
            total = total.checked_add(cost)?;
        }
        Some(total)
    }

    fn field(
        &mut self,
        parent: Option<&'a str>,
        name: &'a str,
        arguments: &Arguments<'a>,
        children: &'a [Selection<'a, DefaultScalarValue>],
        depth: usize,
    ) -> Option<usize> {
        if name == "__typename" {
            return Some(0);
        }
        let depth = depth + 1;
        self.depth = self.depth.max(depth);
        let schema = self.schema;
        let child_type = parent
            .and_then(|parent| schema.concrete_type_by_name(parent))
            .and_then(|parent| parent.field_by_name(name))
            .map(|field| field.field_type.innermost_name());
        let child = self.selections(child_type, children, depth)?;
        let Some(bound) = parent.and_then(|parent| bound(parent, name)) else {
            return child.checked_add(1);
        };
        match self.size(bound, arguments) {
            Ok(size) => match size.checked_mul(child).and_then(|cost| cost.checked_add(1)) {
                Some(cost) if cost <= self.limits.cost => Some(cost),
                _ => {
                    self.errors.push(TOO_COMPLEX.to_owned());
                    Some(0)
                }
            },
            Err(message) => {
                self.errors.push(message);
                Some(0)
            }
        }
    }

    fn size(&self, bound: Bound, arguments: &Arguments<'a>) -> Result<usize, String> {
        let size = match bound {
            Bound::Page(ceiling) => self
                .argument(arguments, "first")
                .and_then(ScalarValue::try_to_int)
                .map_or(Ok(0), |first| effective_page(first, ceiling))
                .map_err(refusal)?
                .into(),
            Bound::Search => i64::from(reader::SEARCH_LIMIT),
            Bound::Labels => match self.list_len(arguments, "ids") {
                Some(len) => {
                    reader::validate_label_count(len).map_err(refusal)?;
                    i64::try_from(len).map_err(|_| TOO_COMPLEX.to_owned())?
                }
                None => 0,
            },
            Bound::Suggestions => reader::suggestion_limit(
                self.argument(arguments, "limit")
                    .and_then(ScalarValue::try_to_int)
                    .map(i64::from),
            ),
            Bound::Operations => return Ok(OPERATION_LIST),
        };
        usize::try_from(size).map_err(|_| TOO_COMPLEX.to_owned())
    }

    fn resolve(
        &self,
        arguments: &Arguments<'a>,
        name: &str,
    ) -> Option<&'a InputValue<DefaultScalarValue>> {
        let (_, value) = arguments.iter().find(|(argument, _)| *argument == name)?;
        match value {
            InputValue::Variable(variable) => self
                .variables
                .get(variable.as_str())
                .or_else(|| self.defaults.get(variable.as_str()).copied()),
            value => Some(value),
        }
    }

    fn argument(&self, arguments: &Arguments<'a>, name: &str) -> Option<&'a DefaultScalarValue> {
        match self.resolve(arguments, name)? {
            InputValue::Scalar(scalar) => Some(scalar),
            _ => None,
        }
    }

    fn list_len(&self, arguments: &Arguments<'a>, name: &str) -> Option<usize> {
        match self.resolve(arguments, name)? {
            InputValue::List(items) => Some(items.len()),
            _ => None,
        }
    }
}

fn refusal(error: AppError) -> String {
    match error {
        AppError::Validation(detail) => detail,
        _ => TOO_COMPLEX.to_owned(),
    }
}

pub(super) fn selected_operation<'a>(
    document: &'a [Definition<'a, DefaultScalarValue>],
    operation_name: Option<&str>,
) -> Option<&'a Operation<'a, DefaultScalarValue>> {
    document
        .iter()
        .filter_map(|definition| match definition {
            Definition::Operation(operation) => Some(&operation.item),
            Definition::Fragment(_) => None,
        })
        .find(|operation| {
            operation_name.is_none()
                || operation.name.as_ref().map(|name| name.item) == operation_name
        })
}

pub(super) fn check(
    schema: &SchemaType<DefaultScalarValue>,
    document: &[Definition<'_, DefaultScalarValue>],
    operation_name: Option<&str>,
    variables: &Variables<DefaultScalarValue>,
    limits: Limits,
) -> Result<(), Vec<String>> {
    let fragments = document
        .iter()
        .filter_map(|definition| match definition {
            Definition::Fragment(fragment) => Some((
                fragment.item.name.item,
                (
                    fragment.item.type_condition.item,
                    fragment.item.selection_set.as_slice(),
                ),
            )),
            Definition::Operation(_) => None,
        })
        .collect();
    let Some(operation) = selected_operation(document, operation_name) else {
        return Ok(());
    };
    let root = match operation.operation_type {
        OperationType::Query => "Query",
        OperationType::Mutation => "Mutation",
        OperationType::Subscription => return Ok(()),
    };
    let defaults = operation
        .variables_definition
        .iter()
        .flat_map(|definitions| definitions.item.items.iter())
        .filter_map(|(name, definition)| {
            definition
                .default_value
                .as_ref()
                .map(|value| (name.item, &value.item))
        })
        .collect();
    let mut analysis = Analysis {
        schema,
        fragments,
        variables,
        defaults,
        limits,
        depth: 0,
        expanding: Vec::new(),
        errors: Vec::new(),
    };
    let cost = analysis.selections(Some(root), &operation.selection_set, 0);
    if cost.is_none_or(|cost| cost > limits.cost) {
        return Err(vec![TOO_COMPLEX.to_owned()]);
    }
    if analysis.depth > limits.depth {
        return Err(vec![TOO_DEEP.to_owned()]);
    }
    if analysis.errors.is_empty() {
        Ok(())
    } else {
        Err(analysis.errors)
    }
}
