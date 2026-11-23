namespace a {
int x;
struct B {
  int m;
};
}
namespace b {
using namespace a;
using a::x;
}
namespace c = a;
struct D : a::B {};
using E = a::B;
