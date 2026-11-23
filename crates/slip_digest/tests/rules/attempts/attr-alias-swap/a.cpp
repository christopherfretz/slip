void f1() {}
void f2() { int y = 0; }
void g() __attribute__((alias("_Z2f1v")));
