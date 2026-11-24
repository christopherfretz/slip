namespace acme { int hook(int x) { return x; } int g(int); }
int u() { return acme::g(1); }
