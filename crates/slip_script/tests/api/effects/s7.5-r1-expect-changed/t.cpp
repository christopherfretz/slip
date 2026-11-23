int g(int);
int f() { return g(1); }
struct B {};
struct D : B {};
void h() {
  struct L {
    int m = g(2);
  };
}
