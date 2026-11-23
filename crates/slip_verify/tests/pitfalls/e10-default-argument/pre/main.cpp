int take(int n = 1);

int caller() { return take(); }

int take(int n) { return n; }
