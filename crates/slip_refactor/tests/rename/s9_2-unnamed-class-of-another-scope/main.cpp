namespace a { struct { int x; } v; }
namespace n { decltype(a::v) v; }
int h() { return n::v.x + a::v.x; }
