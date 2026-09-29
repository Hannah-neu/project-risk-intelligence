interface PageHeaderProps {
  eyebrow?: string;
  title: string;
  description: string;
  aside?: string;
}

export function PageHeader({ eyebrow, title, description, aside }: PageHeaderProps) {
  return (
    <header className="page-header">
      <div>
        {eyebrow && <span className="eyebrow">{eyebrow}</span>}
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      {aside && <div className="header-aside"><span className="live-dot" />{aside}</div>}
    </header>
  );
}
