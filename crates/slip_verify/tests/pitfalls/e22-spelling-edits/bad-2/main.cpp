int pick(int) { return 1; }
int pick(char *) { return 2; }

int caller() { return pick(nullptr); }
