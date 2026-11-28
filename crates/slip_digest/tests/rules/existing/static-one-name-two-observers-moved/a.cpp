namespace legacy { static int helper() { return 2; } int g() { return 3; } }
int u() { return legacy::helper() + legacy::g(); }
