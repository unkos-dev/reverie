import { graphql } from "react-relay";

export const readerWork = graphql`
  fragment readerWork on Work @argumentDefinitions(count: { type: "Int", defaultValue: 5 }) {
    id
    title
    subtitle
    description
    language
    selectedSeries {
      node {
        id
        name
      }
      position
    }
    authors(first: $count) {
      edges {
        cursor
        role
        position
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
    contributors(first: $count) {
      edges {
        cursor
        role
        position
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
    identifiers(first: $count) {
      edges {
        cursor
        node {
          level
          scheme
          value
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
`;

export const readerBook = graphql`
  fragment readerBook on Manifestation
  @argumentDefinitions(
    count: { type: "Int", defaultValue: 5 }
    tagCount: { type: "Int", defaultValue: 10 }
  ) {
    id
    work {
      ...readerWork
    }
    isbn13
    isbn10
    isbnMismatch
    pages
    publisher
    publicationDate
    createdAt
    updatedAt
    coverUrl
    contentRating
    ingestionStatus
    validationStatus
    enrichmentStatus
    metadata {
      workId
      manifestationId
      etag
      title
      subtitle
      description
      language
      isbn10
      isbn13
      pages
      publisher
      publicationDate
      contentRating
    }
    readingState {
      etag
      status
      rating
      notes
      progressPct
      startedAt
      finishedAt
      lastReadAt
    }
    genres(first: $count) {
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
    moods(first: $count) {
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
    tags(first: $tagCount) {
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
    identifiers(first: $count) {
      edges {
        cursor
        node {
          level
          scheme
          value
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    ratings(first: $count) {
      edges {
        cursor
        node {
          source
          rating
          scale
          reviewCount
          fetchedAt
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
`;

export const readerLibraryQuery = graphql`
  query readerLibraryQuery(
    $count: Int = 50
    $after: String
    $filter: BooksFilter
    $sort: [SortLevel!]
  ) {
    books(first: $count, after: $after, filter: $filter, sort: $sort) {
      edges {
        cursor
        node {
          ...readerBook
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
`;

export const readerDetailQuery = graphql`
  query readerDetailQuery($id: ID!) {
    manifestation(id: $id) {
      ...readerBook
    }
  }
`;

export const readerVersionsQuery = graphql`
  query readerVersionsQuery($count: Int = 20, $id: ID!) {
    manifestation(id: $id) {
      id
      versions {
        pendingCount
        acceptedCount
        pending(first: $count) {
          edges {
            cursor
            node {
              id
              fieldName
              source
              newValue
              status
              confidenceScore
              matchType
              observationCount
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

export const readerShelfQuery = graphql`
  query readerShelfQuery($count: Int = 50, $id: ID!, $after: String) {
    shelf(id: $id) {
      id
      name
      isSystem
      itemCount
      createdAt
      updatedAt
      items(first: $count, after: $after) {
        edges {
          cursor
          node {
            ...readerBook
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

export const readerShelvesQuery = graphql`
  query readerShelvesQuery($count: Int = 50, $after: String) {
    shelves(first: $count, after: $after) {
      edges {
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
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

export const readerSeriesQuery = graphql`
  query readerSeriesQuery(
    $manifestationCount: Int = 2
    $count: Int = 50
    $id: ID!
    $after: String
  ) {
    series(id: $id) {
      id
      name
      works(first: $count, after: $after) {
        edges {
          cursor
          position
          node {
            ...readerWork
            manifestations(first: $manifestationCount) {
              edges {
                cursor
                node {
                  ...readerBook
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

export const readerSearchQuery = graphql`
  query readerSearchQuery($q: String!) {
    search(q: $q) {
      snippet
      manifestation {
        ...readerBook
      }
    }
  }
`;

export const readerSuggestionsQuery = graphql`
  query readerSuggestionsQuery($q: String!, $authorIds: [ID!]!, $seriesIds: [ID!]!) {
    authorSuggestions(q: $q) {
      entityId
      value
    }
    seriesSuggestions(q: $q) {
      entityId
      value
    }
    genreSuggestions(q: $q) {
      entityId
      value
    }
    moodSuggestions(q: $q) {
      entityId
      value
    }
    tagSuggestions(q: $q) {
      entityId
      value
    }
    publisherSuggestions(q: $q) {
      entityId
      value
    }
    authorLabels(ids: $authorIds) {
      entityId
      value
    }
    seriesLabels(ids: $seriesIds) {
      entityId
      value
    }
  }
`;

export const readerNodeQuery = graphql`
  query readerNodeQuery($id: ID!) {
    node(id: $id) {
      id
      ... on Manifestation {
        ...readerBook @alias(as: "manifestation")
      }
      ... on Work {
        ...readerWork @alias(as: "work")
      }
      ... on Shelf {
        name
        isSystem
        itemCount
        createdAt
        updatedAt
      }
      ... on Series {
        name
      }
      ... on Viewer {
        displayName
        isChild
      }
    }
  }
`;
