// SPDX-License-Identifier: GPL-2.0-only
/*
 * Secondary CPU boot through the parking protocol mailboxes left by the
 * Windows Phone UEFI on Lumia MSM8x26 devices.
 */

#include <linux/init.h>
#include <linux/io.h>
#include <linux/of.h>
#include <linux/sizes.h>
#include <linux/smp.h>

#include <asm/cacheflush.h>
#include <asm/smp_plat.h>

#define PARK_CPU_ID	0x0
#define PARK_JUMP_LO	0x8
#define PARK_JUMP_HI	0xc

static int park_boot_secondary(unsigned int cpu, struct task_struct *idle)
{
	struct device_node *np = of_get_cpu_node(cpu, NULL);
	void __iomem *mb;
	u32 addr;
	int ret = -ENODEV;

	if (!np)
		return -ENODEV;
	if (of_property_read_u32(np, "cpu-release-addr", &addr)) {
		pr_err("CPU%u: no cpu-release-addr for the parking protocol\n", cpu);
		goto out;
	}

	mb = ioremap(addr, SZ_4K);
	if (!mb) {
		ret = -ENOMEM;
		goto out;
	}

	/* Entry point first, then the CPU id that releases the core */
	writel_relaxed(__pa_symbol(secondary_startup_arm), mb + PARK_JUMP_LO);
	writel_relaxed(0, mb + PARK_JUMP_HI);
	wmb();
	writel_relaxed(cpu_logical_map(cpu) & 0xff, mb + PARK_CPU_ID);
	wmb();
	arch_send_wakeup_ipi_mask(cpumask_of(cpu));
	iounmap(mb);
	pr_info("CPU%u: released from parking mailbox %#x\n", cpu, addr);
	ret = 0;
out:
	of_node_put(np);
	return ret;
}

static const struct smp_operations park_smp_ops __initconst = {
	.smp_boot_secondary	= park_boot_secondary,
};
