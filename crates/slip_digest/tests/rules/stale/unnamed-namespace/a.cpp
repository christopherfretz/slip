namespace legacy { int a; namespace { int b; } }
int u() { return legacy::a + legacy::b; }
