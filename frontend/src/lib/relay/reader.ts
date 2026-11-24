import { graphql } from "react-relay";

export const readerLibrary = graphql`
  fragment readerLibrary on Query
  @argumentDefinitions(
    count: { type: "Int", defaultValue: 50 }
    cursor: { type: "String" }
    sourceKey: { type: "String!" }
    filter: { type: "BooksFilter" }
    sort: { type: "[SortLevel!]" }
  )
  @refetchable(queryName: "readerLibraryPaginationQuery")
  @throwOnFieldError {
    books(first: $count, after: $cursor, filter: $filter, sort: $sort)
      @connection(key: "readerLibrary_books", dynamicKey_UNSTABLE: $sourceKey) {
      edges {
        node {
          id
          metadata {
            title
          }
          ...LibraryTable_rows
          ...BookCard_book
          ...useCellEdit_batchRows
        }
      }
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
  @refetchable(queryName: "readerAuthorEditingPaginationQuery", preferFetchable: true)
  @throwOnFieldError {
    id
    metadata {
      etag
      authors(first: $count, after: $cursor) @connection(key: "readerAuthorEditing_authors") {
        edges {
          node {
            name
          }
        }
      }
    }
  }
`;
