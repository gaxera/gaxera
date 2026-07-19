import sys
markers = ["GAXERA: FACTORY_INVOKED", "GAXERA: INIT_TEST_SUCCESS"]
seen = [False, False]
with open("/home/nyx/.gemini/antigravity-ide/brain/7fc9e3e9-cb93-49a6-b151-aafde63ba3ab/.system_generated/tasks/task-7528.log", "r") as f:
    for line in f:
        for i, m in enumerate(markers):
            if m in line:
                seen[i] = True
print(seen)
