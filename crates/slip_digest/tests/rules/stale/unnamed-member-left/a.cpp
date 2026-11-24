namespace legacy { namespace { int helper() { return 1; } } int a; }
int u() { return legacy::helper() + legacy::a; }
