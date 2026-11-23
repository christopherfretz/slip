struct D {
  ~D();
  template <class T>
  int t();
};
int f(int);
constexpr int c(int x) { return x; }
template <int N>
struct A {};
void ne() noexcept(noexcept(f(3)));
void use(D* p, D& d) {
  p->~D();
  d.template t<int>();
  decltype(f(1)) x = 0;
  A<c(2)> a;
  alignas(c(8)) int z = 0;
}
