enum Color { red, green };
int pick(int) { return 1; }
int pick(...) { return 2; }

int caller() { return pick(red); }
