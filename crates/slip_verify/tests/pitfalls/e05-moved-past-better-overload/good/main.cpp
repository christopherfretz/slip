int h(long) { return 1; }

int other() { return 0; }

int caller() { return h(1); }

int h(int) { return 2; }
