export interface ShowcaseMediaAsset {
  type: 'image' | 'video';
  src: string;
  poster?: string;
  alt?: string;
}

export interface ShowcaseMediaComponent {
  type: 'component';
  render: () => React.ReactNode;
}

export type ShowcaseMedia = ShowcaseMediaAsset | ShowcaseMediaComponent;

export interface ShowcaseStage {
  titleKey: string;
  descriptionKey: string;
  media: ShowcaseMedia;
}

export interface ShowcaseConfig {
  id: string;
  stages: ShowcaseStage[];
}
