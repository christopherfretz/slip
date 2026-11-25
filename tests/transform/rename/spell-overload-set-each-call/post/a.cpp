namespace a {
int g(int x) { return x; }
int g(long x) { return (int)x + 1; }
int g(const char *) { return 2; }
int u() { return g(1) + g(2L) + g("x"); }
}
