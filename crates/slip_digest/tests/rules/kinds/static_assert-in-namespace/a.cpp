namespace acme {
struct S { int x; };
static_assert(sizeof(S) == 4);
}
static_assert(sizeof(acme::S) == 4, "outside");
