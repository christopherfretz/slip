namespace a {
struct C {
  static int count;
  int k() { return count; }
};
int C::count = 0;
}
int u(a::C c) { return a::C::count + c.count; }
