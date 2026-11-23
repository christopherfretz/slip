namespace acme {
namespace inner { int x; }
inline namespace v1 { int y; }
namespace { int z; }
}
int use() { return acme::inner::x + acme::y; }
