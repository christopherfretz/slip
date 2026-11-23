namespace n { void f(int); void f(double); }
namespace m { void f(const char*); }
void h() {
  n::f(1);
  n::f(1.0);
  m::f("s");
}
