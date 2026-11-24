namespace a {
int v = 1;
enum class E { v, x };
int u() { return v + (int)E::v; }
}
