namespace a {
int f(int x) { return x; }
#define NAME f
int u() { return NAME(1); }
}
