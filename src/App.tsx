import { windowLabel } from "./lib/ipc";
import { Overlay } from "./overlay/Overlay";
import { Selector } from "./selector/Selector";

export default function App() {
  return windowLabel() === "selector" ? <Selector /> : <Overlay />;
}
