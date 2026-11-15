import { graphql } from "react-relay";

export const readerLibraryBook = graphql`
  fragment readerLibraryBook on Manifestation {
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
    }
  }
`;

export const readerDetailBook = graphql`
  fragment readerDetailBook on Manifestation {
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
    tags(first: 10) {
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
    versions {
      pendingCount
    }
  }
`;

export const readerLibrary = graphql`
  fragment readerLibrary on Query
  @argumentDefinitions(
    count: { type: "Int", defaultValue: 50 }
    cursor: { type: "String" }
    filter: { type: "BooksFilter" }
    sort: { type: "[SortLevel!]" }
  )
  @refetchable(queryName: "readerLibraryPaginationQuery") {
    books(first: $count, after: $cursor, filter: $filter, sort: $sort)
      @connection(key: "readerLibrary_books") {
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
    $filter: BooksFilter
    $sort: [SortLevel!]
  ) {
    ...readerLibrary @arguments(count: $count, cursor: $cursor, filter: $filter, sort: $sort)
  }
`;

export const readerDetailQuery = graphql`
  query readerDetailQuery($id: ID!) {
    manifestation(id: $id) {
      ...readerDetailBook
    }
  }
`;

export const readerVersions = graphql`
  fragment readerVersions on Manifestation
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
          }
        }
      }
    }
  }
`;

export const readerVersionsQuery = graphql`
  query readerVersionsQuery($id: ID!, $count: Int = 20, $cursor: String) {
    manifestation(id: $id) {
      ...readerVersions @arguments(count: $count, cursor: $cursor)
    }
  }
`;

export const readerShelfItems = graphql`
  fragment readerShelfItems on Shelf
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
  fragment readerShelf on Shelf {
    id
    name
    ...readerShelfItems
  }
`;

export const readerShelfQuery = graphql`
  query readerShelfQuery($id: ID!) {
    shelf(id: $id) {
      ...readerShelf
    }
  }
`;

export const readerShelves = graphql`
  fragment readerShelves on Query
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "readerShelvesPaginationQuery") {
    shelves(first: $count, after: $cursor) @connection(key: "readerShelves_shelves") {
      edges {
        node {
          id
          name
          isSystem
          itemCount
        }
      }
    }
  }
`;

export const readerShelvesQuery = graphql`
  query readerShelvesQuery {
    ...readerShelves
  }
`;

export const readerSeriesWorks = graphql`
  fragment readerSeriesWorks on Series
  @argumentDefinitions(count: { type: "Int", defaultValue: 50 }, cursor: { type: "String" })
  @refetchable(queryName: "readerSeriesWorksPaginationQuery", preferFetchable: true) {
    works(first: $count, after: $cursor) @connection(key: "readerSeriesWorks_works") {
      edges {
        position
        node {
          id
          title
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
  fragment readerSeries on Series {
    id
    name
    ...readerSeriesWorks
  }
`;

export const readerSeriesQuery = graphql`
  query readerSeriesQuery($id: ID!) {
    series(id: $id) {
      ...readerSeries
    }
  }
`;

export const readerSearchHit = graphql`
  fragment readerSearchHit on Manifestation {
    id
    work {
      title
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
    }
  }
`;

export const readerSearchQuery = graphql`
  query readerSearchQuery($q: String!) {
    search(q: $q) {
      snippet
      manifestation {
        ...readerSearchHit
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
    }
  }
`;
