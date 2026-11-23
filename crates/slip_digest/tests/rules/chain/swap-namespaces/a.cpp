namespace p { int v = 1; struct S { int x; }; }
namespace q { int v = 2; struct S { long y; }; }
int use(p::S s, q::S t) { return p::v + s.x + q::v + (int)t.y; }
