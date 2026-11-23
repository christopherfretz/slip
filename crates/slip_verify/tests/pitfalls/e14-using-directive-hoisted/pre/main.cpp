namespace lib {
int g(int) { return 1; }
}
int g(long) { return 2; }

int f() {
  using namespace lib;
  return g(1);
}

int later() { return g(1); }
