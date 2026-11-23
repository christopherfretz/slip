#define REGISTER(x) reg(#x, &x)
int reg(const char *name, int (*)()) { return name[0]; }
int f() { return 1; }

int init() { return REGISTER(f); }
