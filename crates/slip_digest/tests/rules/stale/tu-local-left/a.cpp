namespace legacy { static int helper() { return 1; } int a; }
int u() { return legacy::helper() + legacy::a; }
