int take(int n = 2);

int caller() { return take(); }

int take(int n) { return n; }
