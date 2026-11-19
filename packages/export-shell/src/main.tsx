import { createRoot } from "react-dom/client";
import "@wa/viewer/base.css";
import "@wa/viewer/viewer.css";
import "./shell.css";
import App from "./App";

createRoot(document.getElementById("root")!).render(<App />);
