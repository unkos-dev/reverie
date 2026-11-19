import { graphql } from "react-relay";

export const readerLibraryBook = graphql`
  fragment readerLibraryBook on Manifestation @relay(plural: true) {
    id
    coverUrl
    isbn13
    pages
    createdAt
    work {
      id
      title
      subtitle
      authors(first: 5) {
        edges {
          node {
            name
          }
        }
        pageInfo {
          hasNextPage
          endCursor
        }
      }
      selectedSeries {
        node {
          id
          name
        }
        position
      }
    }
    metadata {
      workId
      manifestationId
      etag
      title
      subtitle
      isbn13
      pages
    }
    readingState {
      etag
      status
      rating
      progressPct
      startedAt
      finishedAt
    }
  }
`;

export const readerWorkAuthors = graphql`
  fragment readerWorkAuthors on Work
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "readerWorkAuthorsPaginationQuery", preferFetchable: true) {
    id
    authors(first: $count, after: $cursor) @connection(key: "readerWorkAuthors_authors") {
      edges {
        node {
          id
          name
        }
      }
    }
  }
`;

export const readerDetailTags = graphql`
  fragment readerDetailTags on Manifestation
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "readerDetailTagsPaginationQuery", preferFetchable: true) {
    id
    tags(first: $count, after: $cursor) @connection(key: "readerDetailTags_tags") {
      edges {
        node {
          id
          name
        }
      }
    }
  }
`;

export const readerDetailBook = graphql`
  fragment readerDetailBook on Manifestation @throwOnFieldError {
    id
    coverUrl
    isbn13
    isbn10
    isbnMismatch
    publisher
    publicationDate
    ingestionStatus
    validationStatus
    enrichmentStatus
    createdAt
    work {
      id
      title
      description
      language
      ...readerWorkAuthors
      selectedSeries {
        node {
          id
          name
        }
        position
      }
    }
    versions {
      pendingCount
    }
    ...readerDetailTags
  }
`;

export const readerLibrary = graphql`
  fragment readerLibrary on Query
  @argumentDefinitions(
    count: { type: "Int", defaultValue: 50 }
    cursor: { type: "String" }
    sourceKey: { type: "String!" }
    filter: { type: "BooksFilter" }
    sort: { type: "[SortLevel!]" }
  )
  @refetchable(queryName: "readerLibraryPaginationQuery", directives: ["@raw_response_type"]) {
    books(first: $count, after: $cursor, filter: $filter, sort: $sort)
      @connection(key: "readerLibrary_books", dynamicKey_UNSTABLE: $sourceKey) {
      edges {
        node {
          ...readerLibraryBook
        }
      }
    }
  }
`;

export const readerLibraryQuery = graphql`
  query readerLibraryQuery(
    $count: Int = 50
    $cursor: String
    $sourceKey: String!
    $filter: BooksFilter
    $sort: [SortLevel!]
  ) @raw_response_type {
    ...readerLibrary
      @arguments(
        count: $count
        cursor: $cursor
        sourceKey: $sourceKey
        filter: $filter
        sort: $sort
      )
  }
`;

export const readerDetailQuery = graphql`
  query readerDetailQuery($id: ID!) @throwOnFieldError {
    manifestation(id: $id) {
      id
      work {
        title
      }
      ...readerDetailBook
    }
  }
`;

export const readerVersions = graphql`
  fragment readerVersions on Manifestation
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "readerVersionsPaginationQuery", preferFetchable: true) {
    id
    versions {
      pendingCount
      pending(first: $count, after: $cursor) @connection(key: "readerVersions_pending") {
        edges {
          node {
            id
            fieldName
            source
            newValue
            confidenceScore
            matchType
            status
            observationCount
          }
        }
      }
    }
  }
`;

export const readerVersionsQuery = graphql`
  query readerVersionsQuery($id: ID!, $count: Int = 20, $cursor: String) @throwOnFieldError {
    manifestation(id: $id) {
      ...readerVersions @arguments(count: $count, cursor: $cursor)
    }
  }
`;

export const readerShelfItems = graphql`
  fragment readerShelfItems on Shelf
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "readerShelfItemsPaginationQuery", preferFetchable: true) {
    items(first: $count, after: $cursor) @connection(key: "readerShelfItems_items") {
      edges {
        node {
          id
        }
      }
    }
  }
`;

export const readerShelf = graphql`
  fragment readerShelf on Shelf @throwOnFieldError {
    id
    name
    isSystem
    itemCount
    createdAt
    updatedAt
    ...readerShelfItems
  }
`;

export const readerShelfQuery = graphql`
  query readerShelfQuery($id: ID!) @throwOnFieldError {
    shelf(id: $id) {
      ...readerShelf
    }
  }
`;

export const readerShelves = graphql`
  fragment readerShelves on Query
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "readerShelvesPaginationQuery") {
    shelves(first: $count, after: $cursor) @connection(key: "readerShelves_shelves") {
      edges {
        node {
          id
          name
          isSystem
          itemCount
          createdAt
          updatedAt
        }
      }
    }
  }
`;

export const readerShelvesQuery = graphql`
  query readerShelvesQuery @throwOnFieldError {
    ...readerShelves
  }
`;

export const readerSeriesWorks = graphql`
  fragment readerSeriesWorks on Series
  @throwOnFieldError
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "readerSeriesWorksPaginationQuery", preferFetchable: true) {
    works(first: $count, after: $cursor) @connection(key: "readerSeriesWorks_works") {
      edges {
        position
        node {
          id
          title
          ...readerWorkAuthors
          manifestations(first: 1) {
            edges {
              node {
                id
                coverUrl
              }
            }
            pageInfo {
              hasNextPage
              endCursor
            }
          }
        }
      }
    }
  }
`;

export const readerSeries = graphql`
  fragment readerSeries on Series @throwOnFieldError {
    id
    name
    ...readerSeriesWorks
  }
`;

export const readerSeriesQuery = graphql`
  query readerSeriesQuery($id: ID!) @throwOnFieldError {
    series(id: $id) {
      id
      name
      ...readerSeries
    }
  }
`;

export const readerAuthorSuggestionsQuery = graphql`
  query readerAuthorSuggestionsQuery($q: String!) {
    authorSuggestions(q: $q) {
      entityId
      value
    }
  }
`;

export const readerSeriesSuggestionsQuery = graphql`
  query readerSeriesSuggestionsQuery($q: String!) {
    seriesSuggestions(q: $q) {
      entityId
      value
    }
  }
`;

export const readerGenreSuggestionsQuery = graphql`
  query readerGenreSuggestionsQuery($q: String!) {
    genreSuggestions(q: $q) {
      entityId
      value
    }
  }
`;

export const readerMoodSuggestionsQuery = graphql`
  query readerMoodSuggestionsQuery($q: String!) {
    moodSuggestions(q: $q) {
      entityId
      value
    }
  }
`;

export const readerTagSuggestionsQuery = graphql`
  query readerTagSuggestionsQuery($q: String!) {
    tagSuggestions(q: $q) {
      entityId
      value
    }
  }
`;

export const readerPublisherSuggestionsQuery = graphql`
  query readerPublisherSuggestionsQuery($q: String!) {
    publisherSuggestions(q: $q) {
      entityId
      value
    }
  }
`;

export const readerAuthorLabelsQuery = graphql`
  query readerAuthorLabelsQuery($ids: [ID!]!) {
    authorLabels(ids: $ids) {
      entityId
      value
    }
  }
`;

export const readerSeriesLabelsQuery = graphql`
  query readerSeriesLabelsQuery($ids: [ID!]!) {
    seriesLabels(ids: $ids) {
      entityId
      value
    }
  }
`;

export const readerNodeQuery = graphql`
  query readerNodeQuery($id: ID!) {
    node(id: $id) {
      id
    }
  }
`;

export const readerAuthorEditing = graphql`
  fragment readerAuthorEditing on Manifestation
  @argumentDefinitions(count: { type: "Int", defaultValue: 20 }, cursor: { type: "String" })
  @refetchable(queryName: "readerAuthorEditingPaginationQuery", preferFetchable: true) {
    id
    metadata {
      workId
      manifestationId
      etag
      authors(first: $count, after: $cursor) @connection(key: "readerAuthorEditing_authors") {
        edges {
          cursor
          node {
            id
            name
          }
        }
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
          endCursor
        }
      }
    }
  }
`;

export const readerLibraryPreferences = graphql`
  fragment readerLibraryPreferences on LibraryPreferences {
    hiddenColumns
    density
    view
    sortStack
    defaults {
      hiddenColumns
      density
      view
      sortStack
    }
  }
`;

export const readerLibraryPreferencesQuery = graphql`
  query readerLibraryPreferencesQuery {
    libraryPreferences {
      ...readerLibraryPreferences
    }
  }
`;
