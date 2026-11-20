import { createRoot } from "react-dom/client";
import "@wk/viewer/base.css";
import "@wk/viewer/viewer.css";
import "./shell.css";
import App from "./App";

createRoot(document.getElementById("root")!).render(<App />);
