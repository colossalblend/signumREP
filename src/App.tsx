import { createHashRouter, Navigate, RouterProvider } from "react-router-dom";
import Layout from "./components/Layout";
import Account from "./pages/Account";
import Catalog from "./pages/Catalog";
import Devices from "./pages/Devices";
import Installs from "./pages/Installs";
import IpaSigning from "./pages/IpaSigning";
import Settings from "./pages/Settings";

const router = createHashRouter([{ path: "/", element: <Layout />, children: [
  { index: true, element: <Catalog /> },
  // Поиск по App Store теперь вкладка Каталога; старые ссылки ведём туда.
  { path: "find", element: <Navigate to="/?scope=store" replace /> },
  { path: "devices", element: <Devices /> },
  { path: "account", element: <Account /> },
  { path: "installs", element: <Installs /> },
  { path: "ipa-signing", element: <IpaSigning /> },
  { path: "settings", element: <Settings /> },
  { path: "*", element: <Navigate to="/" replace /> },
] }]);
export default function App() { return <RouterProvider router={router} />; }
