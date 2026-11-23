#define REGISTER(x) reg(#x, &x)
int reg(const char *name, int (*)()) { return name[0]; }
int start() { return 1; }

int init() { return REGISTER(start); }
