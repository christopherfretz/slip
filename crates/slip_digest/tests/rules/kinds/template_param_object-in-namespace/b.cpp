// std: c++20
namespace legacy {
struct L { int v; };
template <L l> int g() { return l.v; }
}
int use() { return legacy::g<legacy::L{1}>(); }
