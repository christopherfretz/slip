namespace legacy {
struct S { int x; };
static_assert(sizeof(S) == 4);
}
static_assert(sizeof(legacy::S) == 4, "outside");
