int f1() { return 1; }
int f2() { return 2; }
int g() asm("_Z2f1v");
int u() { return g(); }
