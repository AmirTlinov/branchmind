import { Sidebar } from "@/components/layout/Sidebar";
import { CenterArea } from "@/components/layout/CenterArea";
import { Inspector } from "@/components/layout/Inspector";
import { CommandPalette } from "@/components/command/CommandPalette";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { useEffect } from "react";
import { useStore } from "@/store";

export default function App() {
  const init = useStore((s) => s.init);
  useEffect(() => void init(), [init]);

  return (
    <div className="w-full h-screen overflow-hidden font-sans bg-white text-gray-900">
      <PanelGroup direction="horizontal" className="w-full h-full">
        <Panel defaultSize={22} minSize={16} maxSize={34}>
          <Sidebar />
        </Panel>
        <PanelResizeHandle className="w-px bg-gray-200/60 hover:bg-gray-300/70 transition-colors" />
        <Panel minSize={42}>
          <CenterArea />
        </Panel>
        <PanelResizeHandle className="w-px bg-gray-200/60 hover:bg-gray-300/70 transition-colors" />
        <Panel defaultSize={24} minSize={16} maxSize={40}>
          <Inspector />
        </Panel>
      </PanelGroup>

      <CommandPalette />
    </div>
  );
}
