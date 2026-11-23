namespace acme {
struct S { int x; static int s; void f(); };
struct D : S { int y; };
}
acme::D make();
