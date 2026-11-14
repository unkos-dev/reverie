import { graphql } from "react-relay";

export const viewerQuery = graphql`
  query viewerQuery {
    viewer {
      id
      displayName
      isChild
    }
  }
`;
