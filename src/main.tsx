import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { Bar } from "./bar/Bar";
import { Hub } from "./hub/Hub";

const isBar = new URLSearchParams(location.search).get("w") === "bar";
if (isBar) document.body.classList.add("bar");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isBar ? <Bar /> : <Hub />}</React.StrictMode>,
);
