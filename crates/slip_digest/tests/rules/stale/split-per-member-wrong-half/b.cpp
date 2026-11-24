namespace acme { int b; }
namespace acme2 { int a; }
namespace legacy { int c; }
int u() { return acme2::a + acme::b + legacy::c; }
