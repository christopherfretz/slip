namespace legacy { int hook(int x) { return x; } int g(int); }
int u() { return legacy::g(1); }
