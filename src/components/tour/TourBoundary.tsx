import { Component, type ReactNode } from "react";
import { markOnboardingSeen, sendTourSignal, setTourActive } from "../../lib/tourStore";

/**
 * The walkthrough renders on top of the whole app. If it ever throws, the app
 * must stay usable instead of going blank — so a failure here just closes the
 * tour, clears its demo state and lets the user get on with it.
 */
export default class TourBoundary extends Component<
  { children: ReactNode; onClose: () => void },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
    // eslint-disable-next-line no-console
    console.error("onboarding failed", error);
    setTourActive(false);
    sendTourSignal("");
    markOnboardingSeen();
    this.props.onClose();
  }

  render() {
    return this.state.failed ? null : this.props.children;
  }
}
