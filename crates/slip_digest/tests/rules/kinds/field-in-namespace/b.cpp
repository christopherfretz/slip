namespace legacy {
struct S { int a; int b : 3; union { int c; }; };
}
int use(legacy::S s) { return s.a + s.b + s.c; }
