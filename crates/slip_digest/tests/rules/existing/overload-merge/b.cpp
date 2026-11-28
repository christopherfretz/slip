namespace acme { int x(int a) { return a; } int x(double b) { return (int)b; } }
int u() { return acme::x(1) + acme::x(2.0); }
