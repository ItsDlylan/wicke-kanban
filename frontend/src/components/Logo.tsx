export function Logo() {
  return (
    <>
      <img
        src="/wickeban-logo.svg"
        alt="Wickeban"
        width="160"
        className="logo block dark:hidden"
      />
      <img
        src="/wickeban-logo-dark.svg"
        alt="Wickeban"
        width="160"
        className="logo hidden dark:block"
      />
    </>
  );
}
