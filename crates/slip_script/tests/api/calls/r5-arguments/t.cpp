// std: c++23
namespace n {
struct S {
  int m(int a, int b = 2);
  int x(this S& self, int a);
  int k() { return m(1) + x(3); }
};
int f(int a, int b = 2);
int g();
int use(S s) {
  return f(1) + f(1, 2) + s.m(3, 4) + s.x(5) + g();
}
}
