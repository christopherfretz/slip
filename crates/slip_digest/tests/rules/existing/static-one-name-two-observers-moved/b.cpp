namespace acme { static int helper() { return 2; } int g() { return 3; } }
int u() { return acme::helper() + acme::g(); }
