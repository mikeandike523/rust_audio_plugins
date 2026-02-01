type FooterProps = {
  pluginVersion: string | null;
  guiVersion: string;
  loadedFrom: string;
};

export const Footer = ({ pluginVersion, guiVersion, loadedFrom }: FooterProps) => (
  <footer className="footer">
    <div className="version-meta">
      <div>plugin-version: {pluginVersion ?? "unknown"}</div>
      <div>gui-version: {guiVersion}</div>
    </div>
    <div className="source">
      <div className="source-label">Loaded From</div>
      <div className="source-value" title={loadedFrom}>
        {loadedFrom}
      </div>
    </div>
  </footer>
);
