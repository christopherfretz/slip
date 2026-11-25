namespace a {
int f(int x) { return x; }
int u(int f) { return f + 1; }
int w() { return u(f(1)); }
}
