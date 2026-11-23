// std: c++20
namespace acme {
struct L { int v; };
template <L l> int g() { return l.v; }
}
int use() { return acme::g<acme::L{1}>(); }
