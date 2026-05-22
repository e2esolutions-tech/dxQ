import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";

import "./styles.css";
import { AppShell } from "./components/AppShell";
import { InventoryPage } from "./pages/Inventory";
import { PosturePage } from "./pages/Posture";
import { BlockedPage } from "./pages/Blocked";
import { QkdPage } from "./pages/Qkd";
import { RecommendationsPage } from "./pages/Recommendations";
import { RoadmapPage } from "./pages/Roadmap";
import { CompatPage } from "./pages/Compat";
import { DeadlinesPage } from "./pages/Deadlines";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
      <AppShell>
        <Routes>
          <Route path="/" element={<Navigate replace to="/posture" />} />
          <Route path="/posture" element={<PosturePage />} />
          <Route path="/inventory" element={<InventoryPage />} />
          <Route path="/recommendations" element={<RecommendationsPage />} />
          <Route path="/roadmap" element={<RoadmapPage />} />
          <Route path="/compat" element={<CompatPage />} />
          <Route path="/deadlines" element={<DeadlinesPage />} />
          <Route path="/blocked" element={<BlockedPage />} />
          <Route path="/qkd" element={<QkdPage />} />
        </Routes>
      </AppShell>
    </BrowserRouter>
  </StrictMode>,
);
