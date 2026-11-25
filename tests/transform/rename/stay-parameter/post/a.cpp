namespace a {
int g(int x) { return x; }
int u(int f) { return f + 1; }
int w() { return u(g(1)); }
}
