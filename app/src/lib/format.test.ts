import { describe, expect, it } from "vitest";
import { bytes, bytesUp, countWords, day, duration, memory, memoryUp, percent, wordsPerSecond } from "./format";

describe("format", () => {
  it("shows sizes the way storage settings do", () => {
    expect(bytes(2_740_937_888)).toBe("2.7 GB");
    expect(bytes(5_000_000_000)).toBe("5 GB");
    expect(bytes(833_592_096)).toBe("834 MB");
  });

  it("shows installed memory the way it was sold", () => {
    expect(memory(33_378_181_120)).toBe("32 GB"); // what Windows reports for 32 GB
    expect(memory(17_163_091_968)).toBe("16 GB"); // RX 7800 XT
    expect(memory(7.6 * 2 ** 30)).toBe("8 GB");
    expect(memory(5 * 2 ** 30)).toBe("5 GB");
    expect(memoryUp(2.01 * 2 ** 30)).toBe("2.1 GB");
  });

  it("never understates space someone must free", () => {
    expect(bytesUp(3_010_000_000)).toBe("3.1 GB");
    expect(bytesUp(3_000_000_000)).toBe("3 GB");
  });

  it("describes durations in plain words", () => {
    expect(duration(20)).toBe("less than a minute");
    expect(duration(150)).toBe("about 3 minutes");
    expect(duration(4200)).toBe("about 1 hour 10 minutes");
    expect(duration(-1)).toBe("");
  });

  it("rounds measured speeds for humans", () => {
    expect(wordsPerSecond(12.4)).toBe("12");
    expect(wordsPerSecond(3.04)).toBe("3");
    expect(wordsPerSecond(3.46)).toBe("3.5");
  });

  it("clamps progress", () => {
    expect(percent(5, 10)).toBe(50);
    expect(percent(11, 10)).toBe(100);
    expect(percent(1, 0)).toBe(0);
  });

  it("counts words like the engine does", () => {
    expect(countWords("Hello, world — this is *fine*.")).toBe(5);
  });

  it("labels recent days", () => {
    const now = new Date(2026, 8, 24, 12).getTime();
    expect(day(new Date(2026, 8, 24, 8).getTime() / 1000, now)).toBe("Today");
    expect(day(new Date(2026, 8, 23, 23).getTime() / 1000, now)).toBe("Yesterday");
  });
});
