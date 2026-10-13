import { Navigate, type RouteObject } from "react-router";

import App from "./App.tsx";
import { AuthenticatedBoundary } from "./components/AuthenticatedBoundary";
import { RootErrorBoundary } from "./components/RootErrorBoundary";
import {
  adminDashboardRoute,
  adminUsersRoute,
  bookRoute,
  libraryRoute,
  seriesRoute,
  shelfDetailRoute,
  shelvesRoute,
  tokensRoute,
} from "./routes/production";

export const routes: RouteObject[] = [
  // Every route that requires a session. The 401 redirect handler and session
  // recovery live here, so a route outside this layout never redirects.
  {
    element: <AuthenticatedBoundary />,
    errorElement: <RootErrorBoundary />,
    children: [
      {
        path: "/",
        element: <App />,
        errorElement: <RootErrorBoundary />,
        children: [
          { index: true, element: <Navigate to="/library" replace /> },
          libraryRoute,
          bookRoute,
          seriesRoute,
          shelvesRoute,
          shelfDetailRoute,
          tokensRoute,
          adminUsersRoute,
          adminDashboardRoute,
        ],
      },
      {
        path: "/account/password",
        errorElement: <RootErrorBoundary />,
        lazy: async () => {
          const mod = await import("./routes/account-password");
          return { Component: mod.Component };
        },
      },
    ],
  },
  // Pre-auth screens. Siblings of the authenticated boundary, NOT children:
  // they must render without the shell and outside the `useSessionRecovery`
  // redirect funnel, or an unauthenticated visitor would loop. They live at
  // top-level page paths (not under `/auth/*`) so the backend's reserved-prefix
  // fallback serves the SPA for them: `/auth/*` is the auth API/protocol
  // namespace, these are user-facing pages.
  {
    path: "/login",
    errorElement: <RootErrorBoundary />,
    lazy: async () => {
      const mod = await import("./routes/auth-login");
      return { Component: mod.Component };
    },
  },
  {
    path: "/setup",
    errorElement: <RootErrorBoundary />,
    lazy: async () => {
      const mod = await import("./routes/auth-setup");
      return { Component: mod.Component };
    },
  },
  {
    path: "/forgot-password",
    errorElement: <RootErrorBoundary />,
    lazy: async () => {
      const mod = await import("./routes/auth-forgot-password");
      return { Component: mod.Component };
    },
  },
  // No `/register` route on purpose. The self-registration screen and its API
  // client exist and are tested, but registration today is an unmoderated
  // free-for-all (an enabled flag mints immediately-active adult accounts), so
  // account creation stays admin-provisioned. Wire this route back up once
  // registration becomes a request-access flow gated on admin approval.
];
