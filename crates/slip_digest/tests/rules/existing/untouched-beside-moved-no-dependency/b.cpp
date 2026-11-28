namespace acme { int x() { return 1; } int g() { return 2; } }
int u() { return acme::x() + acme::g(); }
