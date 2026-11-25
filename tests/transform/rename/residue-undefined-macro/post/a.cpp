#define IGNORE(x) 0
namespace a { int g(int x) { return x; } }
int u() {
#ifdef NEVER_DEFINED
  return a::f(0);
#endif
  return a::g(1) + IGNORE(a::f(2));
}
