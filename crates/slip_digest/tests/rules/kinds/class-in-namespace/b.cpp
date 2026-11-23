namespace legacy {
struct S { int x; static int s; void f(); };
struct D : S { int y; };
}
legacy::D make();
