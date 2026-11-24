namespace a {
int g(long x) { return (int)x; }
int f(int x) { return x; }
int u() { return g(1) + f(2); }
}
