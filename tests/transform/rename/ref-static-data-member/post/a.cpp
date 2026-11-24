namespace a {
struct C {
  static int total;
  int k() { return total; }
};
int C::total = 0;
}
int u(a::C c) { return a::C::total + c.total; }
