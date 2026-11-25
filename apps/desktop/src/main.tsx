// Mukade on the shell: its identity, its screens, and nothing else. The frame, the palette, the notices, the look and the window are
// Imago's (docs/design-file.md of @acaprino/imago); the parts of every screen are Sigrid's.
import '@acaprino/sigrid/styles.css';
import '@acaprino/sigrid/graph.css';
import { defineApp, startApp } from '@acaprino/imago';
import { Mukade } from './screens/mukade.tsx';

const MUKADE = defineApp({
  id: 'mukade',
  name: 'Mukade',
  lang: 'en',
  // design-check-ignore-next-line color-value: the one colour an app gives, Mukade's hue (the table of the design file)
  accent: '#D0B2FF',
  words: { sigrid: { sidebar: { search: 'Filter the rooms' } } },
  storage: { scheme: 'mukade.theme', zoom: 'mukade.zoom', sidebar: 'mukade.sidebar' },
});

startApp(MUKADE, document.getElementById('app')!, <Mukade />);
