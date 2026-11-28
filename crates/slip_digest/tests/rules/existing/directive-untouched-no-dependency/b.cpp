namespace acme { int x() { return 1; } int g() { return 2; } }
using namespace acme;
int u() { return x() + acme::g(); }
