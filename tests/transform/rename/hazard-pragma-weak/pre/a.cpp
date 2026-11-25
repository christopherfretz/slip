int f(int x) { return x; }
#pragma weak f
int u() { return f(1); }
