namespace n {
struct A {};
enum class E { e };
int f(int);
template <class T>
struct W {
  static int k;
};
template <class T>
concept C = true;
template <C T>
void g(T);
struct P {
  int x;
};
struct S;
}
namespace m {
using n::f;
using enum n::E;
}
using namespace n;
namespace al = n;
int n::f(int y) { return y; }
struct n::S {};
class X;
struct F {
  friend struct n::A;
};
template <>
int n::W<n::A>::k = 0;
n::P p = {.x = 1};
struct n::A* pa;
