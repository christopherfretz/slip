void f2() {}
void f1() { int y = 0; }
void g() __attribute__((alias("_Z2f1v")));
