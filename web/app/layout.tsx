// The root layout: wraps every page. Pages render inside {children}.
import "./styles.css";
import { usePathname, type LayoutProps } from "northstack";

export default function RootLayout({ children }: LayoutProps) {
  const pathname = usePathname();
  const active = (href: string) => (pathname === href ? "active" : undefined);
  return (
    <>
      <nav className="topbar">
        <a href="/" className="brand">
          northstack
        </a>
        <a href="/" className={active("/")}>
          Notes
        </a>
        <a href="/about" className={active("/about")}>
          About
        </a>
      </nav>
      <main>{children}</main>
    </>
  );
}
