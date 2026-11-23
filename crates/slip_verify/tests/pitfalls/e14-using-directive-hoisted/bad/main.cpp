namespace lib {
int g(int) { return 1; }
}
int g(long) { return 2; }
using namespace lib;
int f() {
  return g(1);
}

int later() { return g(1); }
