namespace q { int v = 1; struct S { int x; }; }
namespace p { int v = 2; struct S { long y; }; }
int use(q::S s, p::S t) { return q::v + s.x + p::v + (int)t.y; }
