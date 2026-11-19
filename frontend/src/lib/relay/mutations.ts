import { graphql } from "react-relay";

export const mutationsTitleSnapshot = graphql`
  fragment mutationsTitleSnapshot on MetadataSnapshot {
    workId
    manifestationId
    etag
    title
  }
`;

export const mutationsFormSnapshot = graphql`
  fragment mutationsFormSnapshot on MetadataSnapshot {
    workId
    manifestationId
    etag
    title
    subtitle
    description
    language
    publisher
    publicationDate
    isbn10
    isbn13
    isbnMismatch
    pages
    contentRating
  }
`;

export const mutationsReadingSnapshot = graphql`
  fragment mutationsReadingSnapshot on ReadingState {
    etag
    status
    rating
    notes
    progressPct
    startedAt
    finishedAt
    lastReadAt
  }
`;

export const mutationsTitleQuery = graphql`
  query mutationsTitleQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        ...mutationsTitleSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsSaveTitleMutation = graphql`
  mutation mutationsSaveTitleMutation(
    $id: ID!
    $baseline: String
    $requestId: String
    $patch: MetadataPatch!
  ) {
    updateMetadata(id: $id, baseline: $baseline, requestId: $requestId, patch: $patch) {
      requestId
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsTitleSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsUndoTitleMutation = graphql`
  mutation mutationsUndoTitleMutation($id: ID!, $baseline: String, $operationId: String!) {
    undoMetadata(id: $id, baseline: $baseline, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsTitleSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsResolveTitleQuery = graphql`
  query mutationsResolveTitleQuery($id: ID!, $requestId: String, $operationId: String) {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsTitleSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsFormQuery = graphql`
  query mutationsFormQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        ...mutationsFormSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsSaveFormMutation = graphql`
  mutation mutationsSaveFormMutation(
    $id: ID!
    $baseline: String
    $requestId: String
    $patch: MetadataPatch!
  ) {
    updateMetadata(id: $id, baseline: $baseline, requestId: $requestId, patch: $patch) {
      requestId
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsFormSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsUndoFormMutation = graphql`
  mutation mutationsUndoFormMutation($id: ID!, $baseline: String, $operationId: String!) {
    undoMetadata(id: $id, baseline: $baseline, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsFormSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsResolveFormQuery = graphql`
  query mutationsResolveFormQuery($id: ID!, $requestId: String, $operationId: String) {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsFormSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsRevisionQuery = graphql`
  query mutationsRevisionQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        workId
        manifestationId
        etag
      }
    }
  }
`;

export const mutationsReadingQuery = graphql`
  query mutationsReadingQuery($id: ID!) {
    manifestation(id: $id) {
      id
      readingState {
        ...mutationsReadingSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsReadingMutation = graphql`
  mutation mutationsReadingMutation($id: ID!, $baseline: String, $patch: ReadingPatch!) {
    updateReadingState(id: $id, baseline: $baseline, patch: $patch) {
      ...mutationsReadingSnapshot @relay(mask: false)
    }
  }
`;

export const mutationsAuthorPageQuery = graphql`
  query mutationsAuthorPageQuery($id: ID!, $first: Int!, $after: String) {
    manifestation(id: $id) {
      id
      metadata {
        workId
        manifestationId
        etag
        authors(first: $first, after: $after) {
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
  }
`;

export const mutationsSaveAuthorsMutation = graphql`
  mutation mutationsSaveAuthorsMutation(
    $id: ID!
    $baseline: String
    $requestId: String
    $patch: MetadataPatch!
  ) {
    updateMetadata(id: $id, baseline: $baseline, requestId: $requestId, patch: $patch) {
      requestId
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        workId
        manifestationId
        etag
        authors(first: 20) {
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
  }
`;

export const mutationsUndoAuthorsMutation = graphql`
  mutation mutationsUndoAuthorsMutation($id: ID!, $baseline: String, $operationId: String!) {
    undoMetadata(id: $id, baseline: $baseline, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        workId
        manifestationId
        etag
        authors(first: 20) {
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
  }
`;

export const mutationsResolveAuthorsQuery = graphql`
  query mutationsResolveAuthorsQuery($id: ID!, $requestId: String, $operationId: String) {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        workId
        manifestationId
        etag
        authors(first: 20) {
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
  }
`;

export const mutationsCreateShelfMutation = graphql`
  mutation mutationsCreateShelfMutation($input: CreateShelfInput!, $connections: [ID!]!) {
    createShelf(input: $input) {
      __typename
      ... on ShelfCreated {
        edge @appendEdge(connections: $connections) {
          cursor
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
      ... on ShelfNameInvalid {
        message
      }
    }
  }
`;

export const mutationsRenameShelfMutation = graphql`
  mutation mutationsRenameShelfMutation($input: RenameShelfInput!) {
    renameShelf(input: $input) {
      __typename
      ... on ShelfRenamed {
        shelf {
          id
          name
          updatedAt
        }
      }
      ... on ShelfNameInvalid {
        message
      }
      ... on SystemShelfImmutable {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsDeleteShelfMutation = graphql`
  mutation mutationsDeleteShelfMutation($input: DeleteShelfInput!, $connections: [ID!]!) {
    deleteShelf(input: $input) {
      __typename
      ... on ShelfDeleted {
        deletedShelfId @deleteEdge(connections: $connections)
      }
      ... on SystemShelfImmutable {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsAddShelfItemMutation = graphql`
  mutation mutationsAddShelfItemMutation($input: AddShelfItemInput!, $connections: [ID!]!) {
    addShelfItem(input: $input) {
      __typename
      ... on ShelfItemAdded {
        shelf {
          id
          itemCount
          updatedAt
        }
        edge @appendEdge(connections: $connections) {
          cursor
          node {
            id
          }
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsAcceptMetadataVersionMutation = graphql`
  mutation mutationsAcceptMetadataVersionMutation($id: ID!, $versionId: ID!) {
    acceptMetadataVersion(id: $id, versionId: $versionId) {
      manifestationId
      metadata {
        ...mutationsFormSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsRejectMetadataVersionMutation = graphql`
  mutation mutationsRejectMetadataVersionMutation($id: ID!, $versionId: ID!) {
    rejectMetadataVersion(id: $id, versionId: $versionId) {
      manifestationId
    }
  }
`;

export const mutationsRevertMetadataFieldMutation = graphql`
  mutation mutationsRevertMetadataFieldMutation($id: ID!, $fieldName: String!, $versionId: ID) {
    revertMetadataField(id: $id, fieldName: $fieldName, versionId: $versionId) {
      manifestationId
      metadata {
        ...mutationsFormSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsLockMetadataFieldMutation = graphql`
  mutation mutationsLockMetadataFieldMutation($id: ID!, $fieldName: String!, $entityType: String!) {
    lockMetadataField(id: $id, fieldName: $fieldName, entityType: $entityType) {
      manifestationId
    }
  }
`;

export const mutationsUnlockMetadataFieldMutation = graphql`
  mutation mutationsUnlockMetadataFieldMutation(
    $id: ID!
    $fieldName: String!
    $entityType: String!
  ) {
    unlockMetadataField(id: $id, fieldName: $fieldName, entityType: $entityType) {
      manifestationId
    }
  }
`;

export const mutationsUpdateLibraryPreferencesMutation = graphql`
  mutation mutationsUpdateLibraryPreferencesMutation($patch: LibraryPreferencesPatch!) {
    updateLibraryPreferences(patch: $patch) {
      ...readerLibraryPreferences
    }
  }
`;

export const mutationsRefreshMetadataMutation = graphql`
  mutation mutationsRefreshMetadataMutation($id: ID!) {
    refreshMetadata(id: $id) {
      manifestationId
    }
  }
`;

export const mutationsCellsSnapshot = graphql`
  fragment mutationsCellsSnapshot on MetadataSnapshot {
    workId
    manifestationId
    etag
    title
    subtitle
    isbn13
    pages
  }
`;

export const mutationsCellsQuery = graphql`
  query mutationsCellsQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        ...mutationsCellsSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsSaveCellsMutation = graphql`
  mutation mutationsSaveCellsMutation(
    $id: ID!
    $baseline: String
    $requestId: String
    $patch: MetadataPatch!
  ) {
    updateMetadata(id: $id, baseline: $baseline, requestId: $requestId, patch: $patch) {
      requestId
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsCellsSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsUndoCellsMutation = graphql`
  mutation mutationsUndoCellsMutation($id: ID!, $baseline: String, $operationId: String!) {
    undoMetadata(id: $id, baseline: $baseline, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsCellsSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsResolveCellsQuery = graphql`
  query mutationsResolveCellsQuery($id: ID!, $requestId: String, $operationId: String) {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsCellsSnapshot @relay(mask: false)
      }
    }
  }
`;
