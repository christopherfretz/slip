namespace legacy {
namespace inner { int x; }
inline namespace v1 { int y; }
namespace { int z; }
}
int use() { return legacy::inner::x + legacy::y; }
