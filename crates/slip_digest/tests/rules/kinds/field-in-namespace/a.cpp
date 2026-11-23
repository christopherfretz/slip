namespace acme {
struct S { int a; int b : 3; union { int c; }; };
}
int use(acme::S s) { return s.a + s.b + s.c; }
