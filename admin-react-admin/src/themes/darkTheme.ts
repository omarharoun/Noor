import type { RaThemeOptions } from 'react-admin';

export const darkTheme: RaThemeOptions = {
  palette: {
    mode: 'dark',
    primary: { main: '#90caf9' },
    secondary: { main: '#ce93d8' },
    background: {
      default: '#0a1929',
      paper: '#132f4c',
    },
    text: {
      primary: '#e3f2fd',
      secondary: '#b2bac2',
    },
  },
  components: {
    RaListToolbar: {
      styleOverrides: { root: { alignItems: 'flex-start' } },
    },
    MuiCard: {
      styleOverrides: {
        root: {
          background: 'linear-gradient(135deg, #132f4c 0%, #0a1929 100%)',
          border: '1px solid #1e4976',
          borderRadius: 12,
        },
      },
    },
    MuiTableCell: {
      styleOverrides: {
        root: { borderBottom: '1px solid #1e4976' },
        head: { fontWeight: 700, color: '#90caf9' },
      },
    },
    MuiPaper: {
      styleOverrides: {
        root: { backgroundImage: 'none' },
      },
    },
  },
};
