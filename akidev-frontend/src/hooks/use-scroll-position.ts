import { useEffect, useState } from "react";

/** The window scroll offset, used to make the header solid once the page moves. */
export default function useScrollPosition(): number {
  const [y, setY] = useState(0);

  useEffect(() => {
    const onScroll = () => setY(window.scrollY);
    // Read once on mount: a reload part-way down the page would otherwise leave the
    // header transparent until the first scroll event.
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  return y;
}
