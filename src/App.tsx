import { windowLabel } from "./lib/ipc";
import { Overlay } from "./overlay/Overlay";
import { Selector } from "./selector/Selector";
import { Settings } from "./settings/Settings";

export default function App() {
  switch (windowLabel()) {
    case "selector":
      return <Selector />;
    case "settings":
      return <Settings />;
    default:
      return <Overlay />;
  }
}
