namespace acme { namespace { int helper() { return 1; } } int g() { return 2; } }
int u() { return acme::helper() + acme::g(); }
