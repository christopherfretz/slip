namespace acme { int a; }
namespace acme2 { int b; }
namespace legacy { int c; }
int u() { return acme::a + acme2::b + legacy::c; }
